use sea_orm::{ActiveValue, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, Statement};
use sea_query::{Expr, OnConflict, Query, UnionType};
use shared_constants::permissions::Permission;

use crate::{RepositoryError, entities, repositories::AlwaysCloneableConnection};

#[async_trait::async_trait]
pub trait PermissionRepository: Send + Sync + 'static {
    async fn set_role_permissions(
        &self,
        role: &str,
        permissions: &[Permission],
    ) -> Result<(), RepositoryError>;
    async fn get_role_permissions(
        &self,
        role: &str,
    ) -> Result<Vec<entities::role_permissions::Model>, RepositoryError>;
    async fn set_user_permissions(
        &self,
        user_id: i64,
        to_grant: &[Permission],
        to_revoke: &[Permission],
    ) -> Result<(), RepositoryError>;
    async fn get_user_permissions(
        &self,
        user_id: i64,
    ) -> Result<Vec<entities::user_permissions::Model>, RepositoryError>;
    async fn get_effective_permissions(
        &self,
        user_id: i64,
    ) -> Result<Vec<Permission>, RepositoryError>;
}

pub struct SeaOrmPermissionRepository {
    db: AlwaysCloneableConnection,
}

impl SeaOrmPermissionRepository {
    pub fn new(db: &AlwaysCloneableConnection) -> Self {
        Self { db: db.clone() }
    }
}

#[async_trait::async_trait]
impl PermissionRepository for SeaOrmPermissionRepository {
    async fn set_role_permissions(
        &self,
        role: &str,
        permissions: &[Permission],
    ) -> Result<(), RepositoryError> {
        let to_insert = permissions
            .iter()
            .map(|p| entities::role_permissions::ActiveModel {
                role: ActiveValue::set(role.to_string()),
                permission: ActiveValue::set(p.name.to_string()),
                ..Default::default()
            })
            .collect::<Vec<_>>();

        entities::role_permissions::Entity::insert_many(to_insert)
            .on_conflict(
                OnConflict::columns([
                    entities::role_permissions::Column::Role,
                    entities::role_permissions::Column::Permission,
                ])
                .do_nothing()
                .to_owned(),
            )
            .exec(&self.db)
            .await?;

        Ok(())
    }
    async fn get_role_permissions(
        &self,
        role: &str,
    ) -> Result<Vec<entities::role_permissions::Model>, RepositoryError> {
        entities::role_permissions::Entity::find()
            .filter(entities::role_permissions::Column::Role.eq(role))
            .all(&self.db)
            .await
            .map_err(RepositoryError::from)
    }

    async fn set_user_permissions(
        &self,
        user_id: i64,
        to_grant: &[Permission],
        to_revoke: &[Permission],
    ) -> Result<(), RepositoryError> {
        let mut to_insert = to_grant
            .iter()
            .map(|p| entities::user_permissions::ActiveModel {
                user_id: ActiveValue::set(user_id),
                permission: ActiveValue::set(p.name.to_string()),
                granted: ActiveValue::set(true),
            })
            .collect::<Vec<_>>();

        to_insert.extend(
            to_revoke
                .iter()
                .map(|p| entities::user_permissions::ActiveModel {
                    user_id: ActiveValue::set(user_id),
                    permission: ActiveValue::set(p.name.to_string()),
                    granted: ActiveValue::set(false),
                })
                .collect::<Vec<_>>(),
        );

        entities::user_permissions::Entity::insert_many(to_insert)
            .on_conflict(
                OnConflict::columns([
                    entities::user_permissions::Column::UserId,
                    entities::user_permissions::Column::Permission,
                ])
                .do_nothing()
                .to_owned(),
            )
            .exec(&self.db)
            .await?;

        Ok(())
    }

    async fn get_user_permissions(
        &self,
        user_id: i64,
    ) -> Result<Vec<entities::user_permissions::Model>, RepositoryError> {
        entities::user_permissions::Entity::find()
            .filter(entities::user_permissions::Column::UserId.eq(user_id))
            .all(&self.db)
            .await
            .map_err(RepositoryError::from)
    }

    async fn get_effective_permissions(
        &self,
        user_id: i64,
    ) -> Result<Vec<Permission>, RepositoryError> {
        // i dont think seaorm can do this...
        // but maybe sea_query can
        let granted_query = Query::select()
            .column((
                entities::user_permissions::Entity,
                entities::user_permissions::Column::Permission,
            ))
            .from(entities::user_permissions::Entity)
            .and_where(
                Expr::col((
                    entities::user_permissions::Entity,
                    entities::user_permissions::Column::UserId,
                ))
                .eq(user_id),
            )
            .and_where(
                Expr::col((
                    entities::user_permissions::Entity,
                    entities::user_permissions::Column::Granted,
                ))
                .eq(true),
            )
            .to_owned();

        let revoked_query = Query::select()
            .column((
                entities::user_permissions::Entity,
                entities::user_permissions::Column::Permission,
            ))
            .from(entities::user_permissions::Entity)
            .and_where(
                Expr::col((
                    entities::user_permissions::Entity,
                    entities::user_permissions::Column::UserId,
                ))
                .eq(user_id),
            )
            .and_where(
                Expr::col((
                    entities::user_permissions::Entity,
                    entities::user_permissions::Column::Granted,
                ))
                .eq(false),
            )
            .to_owned();

        let query = sea_query::Query::select()
            .distinct()
            .column((
                entities::permissions::Entity,
                entities::permissions::Column::Name,
            ))
            .from(entities::user_roles::Entity)
            .join(
                sea_orm::JoinType::InnerJoin,
                entities::role_permissions::Entity,
                Expr::col((
                    entities::user_roles::Entity,
                    entities::user_roles::Column::Role,
                ))
                .eq(Expr::col((
                    entities::role_permissions::Entity,
                    entities::role_permissions::Column::Role,
                ))),
            )
            .join(
                sea_orm::JoinType::InnerJoin,
                entities::permissions::Entity,
                Expr::col((
                    entities::role_permissions::Entity,
                    entities::role_permissions::Column::Permission,
                ))
                .eq(Expr::col((
                    entities::permissions::Entity,
                    entities::permissions::Column::Name,
                ))),
            )
            .and_where(
                Expr::col((
                    entities::user_roles::Entity,
                    entities::user_roles::Column::UserId,
                ))
                .eq(user_id),
            )
            .union(UnionType::Distinct, granted_query)
            .union(UnionType::Except, revoked_query)
            .to_owned();

        let (sql, values) = query.build(BoxedQueryBuilder(
            self.db.get_database_backend().get_query_builder(),
        ));

        let result = self
            .db
            .query_all(Statement::from_sql_and_values(
                self.db.get_database_backend(),
                sql,
                values,
            ))
            .await?;

        Ok(result
            .into_iter()
            .map(|r| Permission::from_name(&r.try_get_by_index::<String>(0).unwrap()).unwrap())
            .collect())
    }
}

struct BoxedQueryBuilder(Box<dyn sea_query::QueryBuilder>);

impl sea_query::PrecedenceDecider for BoxedQueryBuilder {
    fn inner_expr_well_known_greater_precedence(
        &self,
        inner: &sea_query::SimpleExpr,
        outer_oper: &sea_query::Oper,
    ) -> bool {
        self.0
            .inner_expr_well_known_greater_precedence(inner, outer_oper)
    }
}

impl sea_query::OperLeftAssocDecider for BoxedQueryBuilder {
    fn well_known_left_associative(&self, op: &sea_query::BinOper) -> bool {
        self.0.well_known_left_associative(op)
    }
}

impl sea_query::QuotedBuilder for BoxedQueryBuilder {
    fn quote(&self) -> sea_query::Quote {
        self.0.quote()
    }
}

impl sea_query::TableRefBuilder for BoxedQueryBuilder {
    fn prepare_table_ref_iden(
        &self,
        table_ref: &sea_query::TableRef,
        sql: &mut dyn sea_query::SqlWriter,
    ) {
        self.0.prepare_table_ref_iden(table_ref, sql);
    }
}

impl sea_query::EscapeBuilder for BoxedQueryBuilder {
    fn escape_string(&self, string: &str) -> String {
        self.0.escape_string(string)
    }

    fn unescape_string(&self, string: &str) -> String {
        self.0.unescape_string(string)
    }
}

impl sea_query::QueryBuilder for BoxedQueryBuilder {
    fn prepare_query_statement(
        &self,
        query: &sea_query::SubQueryStatement,
        sql: &mut dyn sea_query::SqlWriter,
    ) {
        self.0.prepare_query_statement(query, sql);
    }

    fn prepare_value(&self, value: &sea_orm::Value, sql: &mut dyn sea_query::SqlWriter) {
        self.0.prepare_value(value, sql);
    }

    fn insert_default_keyword(&self) -> &str {
        self.0.insert_default_keyword()
    }

    fn insert_default_values(&self, num_rows: u32, sql: &mut dyn sea_query::SqlWriter) {
        self.0.insert_default_values(num_rows, sql);
    }

    fn placeholder(&self) -> (&str, bool) {
        self.0.placeholder()
    }

    fn prepare_bin_oper(&self, bin_oper: &sea_query::BinOper, sql: &mut dyn sea_query::SqlWriter) {
        self.0.prepare_bin_oper(bin_oper, sql);
    }

    fn prepare_bin_oper_common(
        &self,
        bin_oper: &sea_query::BinOper,
        sql: &mut dyn sea_query::SqlWriter,
    ) {
        self.0.prepare_bin_oper_common(bin_oper, sql);
    }

    fn prepare_case_statement(
        &self,
        stmts: &sea_query::CaseStatement,
        sql: &mut dyn sea_query::SqlWriter,
    ) {
        self.0.prepare_case_statement(stmts, sql);
    }

    fn prepare_column_ref(
        &self,
        column_ref: &sea_query::ColumnRef,
        sql: &mut dyn sea_query::SqlWriter,
    ) {
        self.0.prepare_column_ref(column_ref, sql);
    }

    fn prepare_constant(&self, value: &sea_orm::Value, sql: &mut dyn sea_query::SqlWriter) {
        self.0.prepare_constant(value, sql);
    }

    fn prepare_constant_false(&self, sql: &mut dyn sea_query::SqlWriter) {
        self.0.prepare_constant_false(sql);
    }

    fn prepare_constant_true(&self, sql: &mut dyn sea_query::SqlWriter) {
        self.0.prepare_constant_true(sql);
    }

    fn prepare_delete_limit(
        &self,
        delete: &sea_query::DeleteStatement,
        sql: &mut dyn sea_query::SqlWriter,
    ) {
        self.0.prepare_delete_limit(delete, sql);
    }

    fn prepare_delete_order_by(
        &self,
        delete: &sea_query::DeleteStatement,
        sql: &mut dyn sea_query::SqlWriter,
    ) {
        self.0.prepare_delete_order_by(delete, sql);
    }

    fn prepare_delete_statement(
        &self,
        delete: &sea_query::DeleteStatement,
        sql: &mut dyn sea_query::SqlWriter,
    ) {
        self.0.prepare_delete_statement(delete, sql);
    }

    fn prepare_field_order(
        &self,
        order_expr: &sea_query::OrderExpr,
        values: &sea_orm::Values,
        sql: &mut dyn sea_query::SqlWriter,
    ) {
        self.0.prepare_field_order(order_expr, values, sql);
    }

    fn prepare_function_arguments(
        &self,
        func: &sea_query::FunctionCall,
        sql: &mut dyn sea_query::SqlWriter,
    ) {
        self.0.prepare_function_arguments(func, sql);
    }

    fn prepare_function_name(
        &self,
        function: &sea_query::Function,
        sql: &mut dyn sea_query::SqlWriter,
    ) {
        self.0.prepare_function_name(function, sql);
    }

    fn prepare_function_name_common(
        &self,
        function: &sea_query::Function,
        sql: &mut dyn sea_query::SqlWriter,
    ) {
        self.0.prepare_function_name_common(function, sql);
    }

    fn prepare_index_hints(
        &self,
        _select: &sea_query::SelectStatement,
        _sql: &mut dyn sea_query::SqlWriter,
    ) {
        self.0.prepare_index_hints(_select, _sql);
    }

    fn prepare_insert(&self, replace: bool, sql: &mut dyn sea_query::SqlWriter) {
        self.0.prepare_insert(replace, sql);
    }

    fn prepare_insert_statement(
        &self,
        insert: &sea_query::InsertStatement,
        sql: &mut dyn sea_query::SqlWriter,
    ) {
        self.0.prepare_insert_statement(insert, sql);
    }

    fn prepare_join_expr(
        &self,
        join_expr: &sea_query::JoinExpr,
        sql: &mut dyn sea_query::SqlWriter,
    ) {
        self.0.prepare_join_expr(join_expr, sql);
    }

    fn prepare_join_on(&self, join_on: &sea_query::JoinOn, sql: &mut dyn sea_query::SqlWriter) {
        self.0.prepare_join_on(join_on, sql);
    }

    fn prepare_join_table_ref(
        &self,
        join_expr: &sea_query::JoinExpr,
        sql: &mut dyn sea_query::SqlWriter,
    ) {
        self.0.prepare_join_table_ref(join_expr, sql);
    }

    fn prepare_join_type(&self, join_type: &sea_orm::JoinType, sql: &mut dyn sea_query::SqlWriter) {
        self.0.prepare_join_type(join_type, sql);
    }

    fn prepare_join_type_common(
        &self,
        join_type: &sea_orm::JoinType,
        sql: &mut dyn sea_query::SqlWriter,
    ) {
        self.0.prepare_join_type_common(join_type, sql);
    }

    fn prepare_keyword(&self, keyword: &sea_query::Keyword, sql: &mut dyn sea_query::SqlWriter) {
        self.0.prepare_keyword(keyword, sql);
    }

    fn prepare_logical_chain_oper(
        &self,
        log_chain_oper: &sea_query::LogicalChainOper,
        i: usize,
        length: usize,
        sql: &mut dyn sea_query::SqlWriter,
    ) {
        self.0
            .prepare_logical_chain_oper(log_chain_oper, i, length, sql);
    }

    fn prepare_on_conflict_action_common(
        &self,
        on_conflict_action: &Option<sea_query::OnConflictAction>,
        sql: &mut dyn sea_query::SqlWriter,
    ) {
        self.0
            .prepare_on_conflict_action_common(on_conflict_action, sql);
    }

    fn prepare_order(&self, order_expr: &sea_query::OrderExpr, sql: &mut dyn sea_query::SqlWriter) {
        self.0.prepare_order(order_expr, sql);
    }

    fn prepare_order_expr(
        &self,
        order_expr: &sea_query::OrderExpr,
        sql: &mut dyn sea_query::SqlWriter,
    ) {
        self.0.prepare_order_expr(order_expr, sql);
    }

    fn prepare_select_distinct(
        &self,
        select_distinct: &sea_query::SelectDistinct,
        sql: &mut dyn sea_query::SqlWriter,
    ) {
        self.0.prepare_select_distinct(select_distinct, sql);
    }

    fn prepare_select_expr(
        &self,
        select_expr: &sea_query::SelectExpr,
        sql: &mut dyn sea_query::SqlWriter,
    ) {
        self.0.prepare_select_expr(select_expr, sql);
    }

    fn prepare_select_limit_offset(
        &self,
        select: &sea_query::SelectStatement,
        sql: &mut dyn sea_query::SqlWriter,
    ) {
        self.0.prepare_select_limit_offset(select, sql);
    }

    fn prepare_select_lock(
        &self,
        lock: &sea_query::LockClause,
        sql: &mut dyn sea_query::SqlWriter,
    ) {
        self.0.prepare_select_lock(lock, sql);
    }

    fn prepare_select_statement(
        &self,
        select: &sea_query::SelectStatement,
        sql: &mut dyn sea_query::SqlWriter,
    ) {
        self.0.prepare_select_statement(select, sql);
    }

    fn prepare_simple_expr(
        &self,
        simple_expr: &sea_query::SimpleExpr,
        sql: &mut dyn sea_query::SqlWriter,
    ) {
        self.0.prepare_simple_expr(simple_expr, sql);
    }

    fn prepare_simple_expr_common(
        &self,
        simple_expr: &sea_query::SimpleExpr,
        sql: &mut dyn sea_query::SqlWriter,
    ) {
        self.0.prepare_simple_expr_common(simple_expr, sql);
    }

    fn prepare_sub_query_oper(
        &self,
        oper: &sea_query::SubQueryOper,
        sql: &mut dyn sea_query::SqlWriter,
    ) {
        self.0.prepare_sub_query_oper(oper, sql);
    }

    fn prepare_table_ref(
        &self,
        table_ref: &sea_query::TableRef,
        sql: &mut dyn sea_query::SqlWriter,
    ) {
        self.0.prepare_table_ref(table_ref, sql);
    }

    fn prepare_table_sample(
        &self,
        _select: &sea_query::SelectStatement,
        _sql: &mut dyn sea_query::SqlWriter,
    ) {
        self.0.prepare_table_sample(_select, _sql);
    }

    fn prepare_tuple(&self, exprs: &[sea_query::SimpleExpr], sql: &mut dyn sea_query::SqlWriter) {
        self.0.prepare_tuple(exprs, sql);
    }

    fn prepare_un_oper(&self, un_oper: &sea_query::UnOper, sql: &mut dyn sea_query::SqlWriter) {
        self.0.prepare_un_oper(un_oper, sql);
    }

    fn prepare_union_statement(
        &self,
        union_type: UnionType,
        select_statement: &sea_query::SelectStatement,
        sql: &mut dyn sea_query::SqlWriter,
    ) {
        self.0
            .prepare_union_statement(union_type, select_statement, sql);
    }

    fn prepare_update_column(
        &self,
        table_ref: &Option<Box<sea_query::TableRef>>,
        table_refs: &[sea_query::TableRef],
        column: &sea_orm::DynIden,
        sql: &mut dyn sea_query::SqlWriter,
    ) {
        self.0
            .prepare_update_column(table_ref, table_refs, column, sql);
    }

    fn prepare_update_condition(
        &self,
        table_refs: &[sea_query::TableRef],
        condition: &sea_query::ConditionHolder,
        sql: &mut dyn sea_query::SqlWriter,
    ) {
        self.0.prepare_update_condition(table_refs, condition, sql);
    }

    fn prepare_update_from(
        &self,
        from: &[sea_query::TableRef],
        sql: &mut dyn sea_query::SqlWriter,
    ) {
        self.0.prepare_update_from(from, sql);
    }

    fn prepare_update_join(
        &self,
        table_refs: &[sea_query::TableRef],
        condition: &sea_query::ConditionHolder,
        sql: &mut dyn sea_query::SqlWriter,
    ) {
        self.0.prepare_update_join(table_refs, condition, sql);
    }

    fn prepare_update_limit(
        &self,
        update: &sea_query::UpdateStatement,
        sql: &mut dyn sea_query::SqlWriter,
    ) {
        self.0.prepare_update_limit(update, sql);
    }

    fn prepare_update_order_by(
        &self,
        update: &sea_query::UpdateStatement,
        sql: &mut dyn sea_query::SqlWriter,
    ) {
        self.0.prepare_update_order_by(update, sql);
    }

    fn prepare_update_statement(
        &self,
        update: &sea_query::UpdateStatement,
        sql: &mut dyn sea_query::SqlWriter,
    ) {
        self.0.prepare_update_statement(update, sql);
    }

    fn prepare_values_list(
        &self,
        value_tuples: &[sea_query::ValueTuple],
        sql: &mut dyn sea_query::SqlWriter,
    ) {
        self.0.prepare_values_list(value_tuples, sql);
    }

    fn prepare_with_clause(
        &self,
        with_clause: &sea_query::WithClause,
        sql: &mut dyn sea_query::SqlWriter,
    ) {
        self.0.prepare_with_clause(with_clause, sql);
    }

    fn prepare_with_clause_common_tables(
        &self,
        with_clause: &sea_query::WithClause,
        sql: &mut dyn sea_query::SqlWriter,
    ) {
        self.0.prepare_with_clause_common_tables(with_clause, sql);
    }

    fn prepare_with_clause_recursive_options(
        &self,
        with_clause: &sea_query::WithClause,
        sql: &mut dyn sea_query::SqlWriter,
    ) {
        self.0
            .prepare_with_clause_recursive_options(with_clause, sql);
    }

    fn prepare_with_clause_start(
        &self,
        with_clause: &sea_query::WithClause,
        sql: &mut dyn sea_query::SqlWriter,
    ) {
        self.0.prepare_with_clause_start(with_clause, sql);
    }

    fn prepare_with_query(&self, query: &sea_query::WithQuery, sql: &mut dyn sea_query::SqlWriter) {
        self.0.prepare_with_query(query, sql);
    }

    fn prepare_with_query_clause_common_table(
        &self,
        cte: &sea_query::CommonTableExpression,
        sql: &mut dyn sea_query::SqlWriter,
    ) {
        self.0.prepare_with_query_clause_common_table(cte, sql);
    }

    fn prepare_with_query_clause_materialization(
        &self,
        cte: &sea_query::CommonTableExpression,
        sql: &mut dyn sea_query::SqlWriter,
    ) {
        self.0.prepare_with_query_clause_materialization(cte, sql);
    }

    fn value_to_string(&self, v: &sea_orm::Value) -> String {
        self.0.value_to_string(v)
    }

    fn value_to_string_common(&self, v: &sea_orm::Value) -> String {
        self.0.value_to_string_common(v)
    }

    fn values_list_tuple_prefix(&self) -> &str {
        self.0.values_list_tuple_prefix()
    }
}

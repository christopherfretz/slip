#include "side.h"
// §10.1, §2.4 (WP-100): an attribute on a parameter of a block-scope declaration is not the entity's; it is written with that declaration in the block (behind `stmt` here; unit tests `attrs::tests::s10_1_param_chain_leaves_out_block_scope`, `local::tests::a_block_scope_entity_with_linkage_is_a_ref`).
#ifdef B
void f(int *p);
void g() { void f(int *p __attribute__((nonnull))); }
#else
void f(int *p);
void g() { void f(int *p); }
#endif

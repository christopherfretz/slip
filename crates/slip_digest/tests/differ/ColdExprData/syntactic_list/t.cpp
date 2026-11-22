#include "side.h"
// §7.4 ColdExprData `syntactic_list`.
// a list in a template pattern (expr_init.rs)
template<class T> struct A {
#ifdef B
static inline T w[] = {1}; };
#else
static inline T w[] = {1, 2}; };
#endif

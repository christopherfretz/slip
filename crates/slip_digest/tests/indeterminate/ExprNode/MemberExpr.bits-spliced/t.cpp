// std: c++26
#include "side.h"
// §7 ExprNode `MemberExpr.bits-spliced`.
// set only in a template (kiln_sema splice.rs:634)
struct S { int m; };
template<class T> struct A { static inline S s;
static inline int w = s.[:^^S::m:]; };

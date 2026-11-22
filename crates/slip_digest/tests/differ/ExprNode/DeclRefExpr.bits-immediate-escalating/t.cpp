// std: c++26
#include "side.h"
// §7 ExprNode `DeclRefExpr.bits-immediate-escalating`: a constexpr
// function template's specialization escalates to immediate on a call it
// cannot fold (P2564; kiln_sema consteval.rs:1326 marks the callee).
consteval int id(int x) { return x; }
#ifdef B
template<class T> constexpr int f(T x) { return x; }
#else
template<class T> constexpr int f(T x) { return id(x); }
#endif
int v = f(1);

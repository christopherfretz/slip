#include "side.h"
// §7 ExprNode `RequiresExpr.aux`: the stored result of an instantiation; the requirements an instantiation keeps are none.
#ifdef B
struct X {};
template<class T> bool rv = requires(T t) { t + 1; };
bool i = rv<X>;
#else
struct X { int operator+(int); };
template<class T> bool rv = requires(T t) { t + 1; };
bool i = rv<X>;
#endif

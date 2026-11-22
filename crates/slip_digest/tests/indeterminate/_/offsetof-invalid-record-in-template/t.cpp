// errors: yes
#include "side.h"
// §7 ExprNode `OffsetOfExpr.aux`: an invalid record in a default member
// initialiser of a class template, used through an instantiation.
struct S { int n; Undeclared u; int m; };
template <class T> struct G { unsigned long w = __builtin_offsetof(S, m) + sizeof(T); };
unsigned long v = G<int>().w;

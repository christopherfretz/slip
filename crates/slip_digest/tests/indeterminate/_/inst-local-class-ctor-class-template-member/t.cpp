#include "side.h"
// digest §4.3 "in an instantiated body the specialisation": the implicit
// `L()` of a local class with a default member initialiser, in a member
// of a class template's specialisation.
template <class T> struct S { int f() { struct L { int a = 1; }; L l; return l.a; } };
int x = S<int>().f();

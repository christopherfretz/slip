#include "side.h"
// digest §4.3 "What these rules do not yet show": a lambda in the default
// argument of a member of a class template's specialisation. The use
// instantiates the default argument into the function's
// `default_argument` unit (digest-kinds §11.4), which numbering the
// function does not walk: the closure has no number.
template <class T> struct S { int f(int a = [] { return 1; }()) { return a; } };
int x = S<int>().f();

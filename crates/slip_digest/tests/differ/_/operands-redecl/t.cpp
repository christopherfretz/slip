#include "side.h"
namespace lib { struct X {};
#ifndef B
int make(X);
#endif
}
template <class T> int make(T);
lib::X x;
decltype(make(x)) f();
int f() { return 0; }
struct S { decltype(make(x)) m(); static decltype(make(x)) sv; };
int S::m() { return 0; }
int S::sv = 0;
// The operand in a later declaration only: g, S2::sv.
int g();
decltype(make(x)) g() { return 0; }
struct S2 { static int sv; };
decltype(make(x)) S2::sv = 0;

#include "side.h"
// WP-156 fix round 2 (a refusal found by its mutation F11, kept): a
// template-id with an operand in an instantiated body (`Q3<T,
// decltype(make(x))>` with T = int) names `Q3<int, int>`; Kiln substitutes
// T without leaving a substituted parameter in that type, so it is an
// unlinked held type naming the class, and the member specialisation
// `Q3<int, int>::s`, written with no operand, fails closed
// (`kiln/declarator-qualifier`): a correct transform refused, which the
// declarator's qualifier as written (KT1 2.3) closes.
namespace lib { struct X {};
#ifndef B
int make(X);
#endif
}
template <class T> int make(T);
lib::X x;
template <class T, class U> struct Q3 { static int s; };
template <class T, class U> int Q3<T, U>::s = 0;
template <class T> int f() { return sizeof(Q3<T, decltype(make(x))>); }
int z = f<int>();
template <> int Q3<int, int>::s = 1;

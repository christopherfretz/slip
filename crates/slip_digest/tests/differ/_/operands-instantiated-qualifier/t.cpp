#include "side.h"
// WP-156 fix round 2 (F11), fix round 3 (moved from `indeterminate`: fix round 2's STOP (b) is a read): the member specialisation `Q3<int, int>::s`, written with no operand, of a class an instantiated body names through a template-id with an operand. It writes its qualifier's entries (T2), equal on both sides; `make(x)` binds otherwise in `f<int>`'s body, which its stream walks.
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

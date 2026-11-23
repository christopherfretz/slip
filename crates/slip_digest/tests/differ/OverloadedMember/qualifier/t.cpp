#include "side.h"
// §7.4 OverloadedMember `qualifier`: `p->Dq::f(x<T>)` against
// `p->f(x<T>)` in a template where the access stays overloaded. The
// naming class and the first candidate are the same on both sides; the
// qualifier is what suppresses virtual dispatch ([class.virtual]/16).
struct Dq { virtual int f(int); virtual int f(long); };
Dq *p;
template <class T> T x{};
#ifdef B
template <class T> auto v = p->Dq::f(x<T>);
#else
template <class T> auto v = p->f(x<T>);
#endif

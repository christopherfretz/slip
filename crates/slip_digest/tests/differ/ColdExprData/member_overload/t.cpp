#include "side.h"
// §7.4 ColdExprData `member_overload`: an access to an overloaded member
// that stays overloaded in a template (the argument is dependent) keeps
// its names. The first candidate is `Dm::f(int)` on both sides; only the
// record tells a qualified access, which is not dispatched, from one
// that is.
struct Dm { virtual int f(int); virtual int f(long); };
Dm *p;
template <class T> T x{};
#ifdef B
template <class T> auto v = p->Dm::f(x<T>);
#else
template <class T> auto v = p->f(x<T>);
#endif

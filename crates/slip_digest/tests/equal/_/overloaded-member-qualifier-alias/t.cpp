#include "side.h"
// §7.4 OverloadedMember `qualifier`: a qualifier is written by what it
// names, so naming the class through an alias does not change an
// overloaded access in a template.
struct Bc { int f(int); int f(long); };
using Al = Bc;
Bc *p;
template <class T> T x{};
#ifdef B
template <class T> auto v = p->Al::f(x<T>);
#else
template <class T> auto v = p->Bc::f(x<T>);
#endif

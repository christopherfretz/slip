#include "side.h"
// §4.4, §7 _ `dependent-call-adl`: `f(t)` against `ns::f(t)` for the same set: argument-dependent lookup.
namespace ns { int f(int); int f(long); }
using ns::f;
#ifdef B
template<class T> int v = ns::f(T());
#else
template<class T> int v = f(T());
#endif

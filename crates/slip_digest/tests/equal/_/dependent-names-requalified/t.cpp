#include "side.h"
// §4.4, §7 _ `dependent-names-requalified`: every dependent name requalified.
namespace ns { template<class T> int g(T); template<class T> concept K = true; int f(int); int f(long); }
#ifdef B
template<class T> int v = ::ns::f(T()) + ::ns::g(T()) + T::x;
#else
template<class T> int v = ns::f(T()) + ns::g(T()) + T::x;
#endif

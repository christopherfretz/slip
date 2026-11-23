#include "side.h"
// §4.4, §7 _ `decltype-requalified`: the same profile requalified gives the same key.
namespace ns { template<class T> int g(T); template<class T> concept K = true; int f(int); int f(long); }
#ifdef B
template<class T> auto f(T a) -> decltype(::ns::g(a));
#else
template<class T> auto f(T a) -> decltype(ns::g(a));
#endif

#include "side.h"
// §4.4, §7 _ `template-parameters-renamed`: template parameters renamed.
namespace ns { template<class T> int g(T); template<class T> concept K = true; int f(int); int f(long); }
#ifdef B
template<class U> int v = ns::f(U()) + U::x;
#else
template<class T> int v = ns::f(T()) + T::x;
#endif

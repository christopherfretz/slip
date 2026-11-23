#include "side.h"
// §4.4, §7 _ `requires-clause-requalified`: a requires-clause's atomic constraint requalified.
namespace ns { template<class T> int g(T); template<class T> concept K = true; int f(int); int f(long); }
#ifdef B
template<class T> requires ::ns::K<T> int v = 1;
#else
template<class T> requires ns::K<T> int v = 1;
#endif

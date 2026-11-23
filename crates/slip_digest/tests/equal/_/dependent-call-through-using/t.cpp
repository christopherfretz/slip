#include "side.h"
// §4.4, §7 _ `dependent-call-through-using`: the same set found through a using-declaration (before `v`) and directly (the using-declaration after `v`).
namespace ns { int f(int); int f(long); }
using namespace ns;
#ifdef B
namespace m { template<class T> int v = f(T()); using ns::f; }
#else
namespace m { using ns::f; template<class T> int v = f(T()); }
#endif

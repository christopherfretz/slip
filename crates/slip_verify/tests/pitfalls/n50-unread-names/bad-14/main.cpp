#include "dep.h"

struct X { int b; };

namespace acme {
template <int N> struct Q { static int s; };
template <int N> int Q<N>::s = 0;
template <> int Q<sizeof(X)>::s = 1;
}

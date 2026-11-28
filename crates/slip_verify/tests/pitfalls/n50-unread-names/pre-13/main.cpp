#include "dep.h"

struct X { int b; };

namespace legacy {
template <int N> int at() { return N; }
template int at<sizeof(X)>();
}

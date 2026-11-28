#include "dep.h"

struct X { int b; };

namespace acme {
template <int N> int at() { return N; }
template int at<sizeof(legacy::X)>();
}

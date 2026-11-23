#include "dep.h"
const int limit = 4;
namespace legacy {
template <int N = limit> int take() { return N; }
}

int caller() { return legacy::take(); }

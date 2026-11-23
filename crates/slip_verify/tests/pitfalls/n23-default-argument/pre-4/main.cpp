#include "dep.h"
const int limit = 4;
namespace legacy {
template <class T> T take(T n = limit) { return n; }
}

int caller() { return legacy::take<int>(); }

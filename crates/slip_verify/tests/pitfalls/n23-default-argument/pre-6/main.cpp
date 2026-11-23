#include "dep.h"
const int limit = 4;
namespace legacy {
int take(...) { return 2; }
template <class T> requires (limit > 5) int take(T) { return 1; }
}

int caller() { return legacy::take(0); }

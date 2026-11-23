#include "dep.h"
const int limit = 4;
namespace acme {
template <class T> T take(T n = limit) { return n; }
}

int caller() { return acme::take<int>(); }

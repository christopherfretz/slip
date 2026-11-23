#include "dep.h"
const int limit = 4;
namespace acme {
int take(...) { return 2; }
template <class T> requires (legacy::limit > 5) int take(T) { return 1; }
}

int caller() { return acme::take(0); }

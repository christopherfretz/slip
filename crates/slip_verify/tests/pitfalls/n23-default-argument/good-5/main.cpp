#include "dep.h"
const int limit = 4;
namespace acme {
template <int N = legacy::limit> int take() { return N; }
}

int caller() { return acme::take(); }

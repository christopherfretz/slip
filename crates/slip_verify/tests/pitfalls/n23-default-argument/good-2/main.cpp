#include "dep.h"
const int limit = 4;
namespace acme {
int take(int n = legacy::limit) { return n; }
}

int caller() { return acme::take(); }

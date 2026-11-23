#include "dep.h"

namespace acme {
int take(int n = legacy::limit) { return n; }
}

int caller() { return acme::take(); }

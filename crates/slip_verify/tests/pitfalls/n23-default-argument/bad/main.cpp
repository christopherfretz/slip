#include "dep.h"

namespace acme {
int take(int n = limit) { return n; }
}

int caller() { return acme::take(); }

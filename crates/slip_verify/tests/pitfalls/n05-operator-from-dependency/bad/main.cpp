#include "dep.h"

namespace acme {
struct Mine {};

}

bool caller(acme::Mine a, acme::Mine b) { return a == b; }

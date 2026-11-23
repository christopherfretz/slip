#include "dep.h"

namespace acme {
struct Mine { operator int() const { return 0; } };
using legacy::operator==;
}

bool caller(acme::Mine a, acme::Mine b) { return a == b; }

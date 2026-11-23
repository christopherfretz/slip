#include "dep.h"

namespace acme {
struct Mine {};
using legacy::operator==;
}

bool caller(acme::Mine a, acme::Mine b) { return a == b; }

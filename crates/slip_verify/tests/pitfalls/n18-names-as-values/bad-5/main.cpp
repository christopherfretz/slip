#include "dep.h"

namespace acme {
int where() { return legacy::length(__PRETTY_FUNCTION__); }
}

int user() { return acme::where(); }

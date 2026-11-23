#include "dep.h"

namespace legacy {
int where() { return legacy::length(__PRETTY_FUNCTION__); }
}

int user() { return legacy::where(); }

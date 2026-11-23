#include "dep.h"

namespace legacy {
struct Mine { operator int() const { return 0; } };

}

bool caller(legacy::Mine a, legacy::Mine b) { return a == b; }

#include "dep.h"

namespace legacy {
struct Mine {};

}

bool caller(legacy::Mine a, legacy::Mine b) { return a == b; }

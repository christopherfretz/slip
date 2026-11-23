#include "dep.h"

namespace legacy {
struct Mine {};
}

using legacy::Mine;

void user() { Mine m; }

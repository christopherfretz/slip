#include "dep.h"

namespace legacy {
inline namespace v2 {
struct Mine {};
}
}

void user() { legacy::Mine m; }

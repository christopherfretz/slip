#include "dep.h"

namespace legacy {
inline namespace v1 {
struct Mine {};
}
}

void user() { legacy::Mine m; }

#include "dep.h"

namespace acme {
inline namespace v1 {
struct Mine {};
}
}

void user() { acme::Mine m; }

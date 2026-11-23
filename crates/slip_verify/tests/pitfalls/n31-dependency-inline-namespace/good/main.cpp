#include "dep.h"

namespace acme {
inline namespace v2 {
struct Mine {};
}
}

void user() { acme::Mine m; }

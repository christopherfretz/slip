#include "dep.h"

namespace acme {
namespace v1 {
struct Mine {};
}
}

void user() { acme::v1::Mine m; }

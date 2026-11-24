#include "dep.h"

namespace acme {
namespace v1 {
int g(int x) { return x; }
}
}

int user() { return acme::v1::g(1); }

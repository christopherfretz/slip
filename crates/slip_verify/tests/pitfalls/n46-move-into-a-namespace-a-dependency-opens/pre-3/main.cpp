#include "dep.h"

namespace legacy {
namespace v1 {
int g(int x) { return x; }
}
}

int user() { return legacy::v1::g(1); }

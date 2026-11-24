#include "dep.h"

namespace acme {
int g(int x) { return x; }
}
namespace legacy {
int h(int x) { return x; }
}

int user() { return acme::g(1) + legacy::h(2); }

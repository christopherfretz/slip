#include "dep.h"

namespace acme {
struct Mine {};
}

namespace legacy { using acme::Mine; }
int user() { return ENROLL(legacy::Mine); }

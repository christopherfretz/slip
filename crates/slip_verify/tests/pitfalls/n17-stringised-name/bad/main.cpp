#include "dep.h"

namespace acme {
struct Mine {};
}


int user() { return ENROLL(acme::Mine); }

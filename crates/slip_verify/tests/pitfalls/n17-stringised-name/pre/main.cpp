#include "dep.h"

namespace legacy {
struct Mine {};
}


int user() { return ENROLL(legacy::Mine); }

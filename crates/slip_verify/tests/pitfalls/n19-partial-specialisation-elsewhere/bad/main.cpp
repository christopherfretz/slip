#include "dep.h"
#include "box_ptr.h"

int *user(acme::box<int *> b) { return b.p; }

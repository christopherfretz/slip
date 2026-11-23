#include "dep.h"
#include "box_ptr.h"

int *user(legacy::box<int *> b) { return b.p; }

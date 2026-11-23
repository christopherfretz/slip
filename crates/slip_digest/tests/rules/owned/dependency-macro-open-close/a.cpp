#include "dep.h"
DEP_OPEN
struct Mine {};
DEP_CLOSE
int use() { return sizeof(legacy::Mine); }

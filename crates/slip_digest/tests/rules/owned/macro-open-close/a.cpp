#include "own.h"
OPEN_LEGACY
struct Mine {};
CLOSE
int use() { return sizeof(legacy::Mine); }

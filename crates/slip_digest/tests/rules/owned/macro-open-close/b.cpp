#include "own.h"
OPEN_ACME
struct Mine {};
CLOSE
int use() { return sizeof(acme::Mine); }

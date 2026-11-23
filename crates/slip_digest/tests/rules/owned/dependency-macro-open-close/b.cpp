#include "dep.h"
namespace acme {
struct Mine {};
DEP_CLOSE
int use() { return sizeof(acme::Mine); }

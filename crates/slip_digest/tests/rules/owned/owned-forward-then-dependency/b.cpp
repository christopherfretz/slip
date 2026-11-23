#include "base.h"
namespace acme { struct Late; }
#undef LEGACY_BEGIN
#define LEGACY_BEGIN namespace acme {
#include "dep.h"
int use() { return sizeof(acme::Late); }

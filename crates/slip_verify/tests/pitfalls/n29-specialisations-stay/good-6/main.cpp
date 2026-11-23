#include "dep.h"

namespace acme {
struct Mine {};
} namespace legacy {
template struct vec<acme::Mine>;
}

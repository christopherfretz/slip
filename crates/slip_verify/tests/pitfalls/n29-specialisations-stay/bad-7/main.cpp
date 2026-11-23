#include "dep.h"

namespace acme {
struct Mine {};
extern template struct vec<Mine>;
}

int user() { legacy::vec<acme::Mine> v; return v.size(); }

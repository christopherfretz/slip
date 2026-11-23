#include "dep.h"

namespace acme {
struct Mine {};
} namespace legacy {
extern template struct vec<acme::Mine>;
}

int user() { legacy::vec<acme::Mine> v; return v.size(); }

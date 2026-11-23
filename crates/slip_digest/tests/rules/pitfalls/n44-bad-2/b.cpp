namespace legacy {
struct Dep {};
}

namespace acme {
namespace {
int base() { return 2; }
}
inline int helper() { return 1; }
int api() { return helper() + base(); }
}

int user() { return acme::api(); }

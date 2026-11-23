namespace n { inline int f() { return 1; } }
namespace m { }
int u() { return n::f(); }

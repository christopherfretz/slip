namespace legacy { namespace { int helper() { return 1; } } }
namespace acme { int a; }
int u() { return legacy::helper() + acme::a; }

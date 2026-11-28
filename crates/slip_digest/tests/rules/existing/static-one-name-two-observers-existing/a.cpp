namespace acme { static int helper() { return 1; } }
namespace legacy { int g() { return 2; } }
int u() { return acme::helper() + legacy::g(); }

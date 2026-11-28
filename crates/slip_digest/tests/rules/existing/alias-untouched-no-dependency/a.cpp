namespace acme { int x() { return 1; } }
namespace legacy { int g() { return 2; } }
namespace A = acme;
int u() { return A::x() + legacy::g(); }

namespace acme { int x() { return 1; } int g() { return 2; } }
namespace A = acme;
int u() { return A::x() + acme::g(); }

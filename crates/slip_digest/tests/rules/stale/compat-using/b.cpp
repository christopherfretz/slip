namespace acme { int f(int); }
namespace legacy { using acme::f; }
int u() { return acme::f(1); }

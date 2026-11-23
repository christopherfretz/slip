namespace n { struct S { friend int f(S); }; }
namespace __attribute__((visibility("hidden"))) n { }
int n::f(S) { return 1; }

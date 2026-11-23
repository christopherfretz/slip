namespace n { struct S { friend int f(S); }; }
namespace n { }
int n::f(S) { return 1; }

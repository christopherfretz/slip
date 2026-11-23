struct S { int a; int b; };
int use(S s) { return s.a; }
int S::*pm = &S::a;

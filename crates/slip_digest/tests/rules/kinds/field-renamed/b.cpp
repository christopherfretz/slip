struct S { int z; int b; };
int use(S s) { return s.z; }
int S::*pm = &S::z;

struct S { static int v; };
int S::v = 1;
int use() { return S::v; }

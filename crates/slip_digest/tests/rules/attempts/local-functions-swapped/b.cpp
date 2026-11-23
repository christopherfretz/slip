int g() { struct L { int a; }; return L{1}.a; }
int f() { struct L { long a; }; return (int)L{2}.a; }
int use() { return g() + f(); }

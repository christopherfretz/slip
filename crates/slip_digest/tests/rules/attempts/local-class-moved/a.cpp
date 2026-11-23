int f() { struct L { int a; }; return L{1}.a; }
int g() { struct L { long a; }; return (int)L{2}.a; }

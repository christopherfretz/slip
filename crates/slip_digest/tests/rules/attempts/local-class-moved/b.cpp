int f() { struct L { long a; }; return (int)L{1}.a; }
int g() { struct L { int a; }; return L{2}.a; }

import re, html, sys, subprocess, os, pathlib
UA = "slippypack-compliance-research/1.0 (toby.murray@brilliantmade.com)"
def get(url, name):
    p = pathlib.Path("terms")/f"{name}.html"
    if not p.exists():
        subprocess.run(["curl","-sSL","--max-time","60","-A",UA,url,"-o",str(p)],check=False)
    t = p.read_text(encoding="utf-8", errors="replace")
    t = re.sub(r'(?is)<(script|style|noscript|svg).*?</\1>','',t)
    t = re.sub(r'(?s)<[^>]+>','\n',t)
    t = html.unescape(t)
    lines=[l.strip() for l in t.split('\n') if l.strip()]
    return '\n'.join(lines)
if __name__=="__main__":
    print(get(sys.argv[1], sys.argv[2]))

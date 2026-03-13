import codebones

# Initialize codebones and run an index on the current directory
print("Testing codebones...")
try:
    codebones.Codebones.index(".")
    results = codebones.Codebones.search("codebones")
    print(f"Found {len(results)} symbols matching 'codebones'.")
    print("Success!")
except Exception as e:
    print(f"Failed: {e}")

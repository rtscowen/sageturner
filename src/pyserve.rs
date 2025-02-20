// currently must be in same directory as dockerfile
pub fn get_serve_code(filename: &str) -> String {
    let content = format!(r#"
    import {} as user_code_file
    from fastapi import FastAPI
    import uvicorn
    model = user_code_file.load()
    app = FastAPI()
    @app.post('/ping')
    def ping():
        if model: 
            return 
        else:
            raise HTTPException(status_code=500, detail="Error")
    @app.post('/invocations')
    def predict(request):
        response = predict(request)
        return response
    if __name__ == "__main__":
         config = uvicorn.Config("serve:app", port=8080)
         server = uvicorn.Server(config=config)
         server.run()
    "#, filename);
    return content;
}
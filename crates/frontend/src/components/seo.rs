use leptos::*;

#[component]
pub fn MetaTags(
    title: &'static str,
    description: &'static str,
    #[prop(optional)]
    image_url: Option<&'static str>,
) -> impl IntoView {
    let full_title = if title == "pointe.dev" {
        "pointe.dev".to_string()
    } else {
        format!("{} | pointe.dev", title)
    };
    
    view! {
        <meta name="title" content=&full_title />
        <meta name="description" content=description />
        <meta name="viewport" content="width=device-width, initial-scale=1" />
        <meta name="robots" content="index, follow" />
        <meta name="language" content="en" />
        <meta name="author" content="pointe.dev" />
        
        <meta property="og:type" content="website" />
        <meta property="og:url" content="https://pointe.dev" />
        <meta property="og:title" content=&full_title />
        <meta property="og:description" content=description />
        {image_url.map(|img| {
            view! {
                <meta property="og:image" content=img />
            }.into_view()
        })}
        <meta property="og:site_name" content="pointe.dev" />
        <meta property="og:locale" content="en_US" />
        
        <meta property="twitter:card" content="summary_large_image" />
        <meta property="twitter:url" content="https://pointe.dev" />
        <meta property="twitter:title" content=&full_title />
        <meta property="twitter:description" content=description />
        {image_url.map(|img| {
            view! {
                <meta property="twitter:image" content=img />
            }.into_view()
        })}
        
        <link rel="canonical" href="https://pointe.dev" />
        
        <meta name="theme-color" content="#0B0B0B" />
        <link rel="icon" type="image/svg+xml" href="/favicon.svg" />
    }
}

/// Render JSON-LD structured data for Organization
pub fn organization_schema() -> String {
    r#"{
        "@context": "https://schema.org",
        "@type": "Organization",
        "name": "pointe.dev",
        "url": "https://pointe.dev",
        "logo": "https://pointe.dev/logo.png",
        "description": "Premium AI product commercialization and business process automation agency",
        "contact": {
            "@type": "ContactPoint",
            "contactType": "Customer Service",
            "email": "hello@pointe.dev"
        },
        "sameAs": [
            "https://twitter.com/pointedev",
            "https://linkedin.com/company/pointedev"
        ]
    }"#.to_string()
}

/// Render JSON-LD structured data for LocalBusiness
pub fn local_business_schema() -> String {
    r#"{
        "@context": "https://schema.org",
        "@type": "LocalBusiness",
        "name": "pointe.dev",
        "image": "https://pointe.dev/og-image.png",
        "description": "Enterprise-grade AI automation and custom solutions",
        "url": "https://pointe.dev",
        "telephone": "+1-XXX-XXX-XXXX",
        "address": {
            "@type": "PostalAddress",
            "streetAddress": "",
            "addressLocality": "",
            "addressRegion": "",
            "postalCode": "",
            "addressCountry": "US"
        },
        "priceRange": "$$$"
    }"#.to_string()
}

/// Render JSON-LD structured data for Service
pub fn service_schema(name: &str, description: &str) -> String {
    format!(
        r#"{{
        "@context": "https://schema.org",
        "@type": "Service",
        "name": "{}",
        "description": "{}",
        "provider": {{
            "@type": "Organization",
            "name": "pointe.dev",
            "url": "https://pointe.dev"
        }}
    }}"#,
        name, description
    )
}
